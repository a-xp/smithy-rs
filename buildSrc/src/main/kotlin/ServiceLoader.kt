/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0.
 */

import org.gradle.api.Project
import software.amazon.smithy.aws.traits.ServiceTrait
import software.amazon.smithy.model.Model
import software.amazon.smithy.model.shapes.ServiceShape
import software.amazon.smithy.model.traits.TitleTrait
import java.io.File
import kotlin.streams.toList

/**
 * Discovers services from the `aws-models` directory within the project.
 *
 * Since this function parses all models, it is relatively expensive to call. The result should be cached in a property
 * during build.
 */
fun Project.discoverServices(serviceMembership: Membership): List<AwsService> {
    val models = project.file("aws-models")
    val baseServices = fileTree(models)
        .sortedBy { file -> file.name }
        .mapNotNull { file ->
            val model = Model.assembler().addImport(file.absolutePath).assemble().result.get()
            val services: List<ServiceShape> = model.shapes(ServiceShape::class.java).sorted().toList()
            if (services.size > 1) {
                throw Exception("There must be exactly one service in each aws model file")
            }
            if (services.isEmpty()) {
                logger.info("${file.name} has no services")
                null
            } else {
                val service = services[0]
                val title = service.expectTrait(TitleTrait::class.java).value
                val sdkId = service.expectTrait(ServiceTrait::class.java).sdkId
                    .toLowerCase()
                    .replace(" ", "")
                    // TODO: the smithy models should not include the suffix "service"
                    .removeSuffix("service")
                    .removeSuffix("api")
                val testFile = file.parentFile.resolve("$sdkId-tests.smithy")
                val extras = if (testFile.exists()) {
                    logger.warn("Discovered protocol tests for ${file.name}")
                    listOf(testFile)
                } else {
                    listOf()
                }
                AwsService(
                    service = service.id.toString(),
                    module = sdkId,
                    moduleDescription = "AWS SDK for $title",
                    modelFile = file,
                    extraFiles = extras,
                    humanName = title
                )
            }
        }
    val baseModules = baseServices.map { it.module }.toSet()

    // validate the full exclusion list hits
    serviceMembership.exclusions.forEach { disabledService ->
        check(baseModules.contains(disabledService)) {
            "Service $disabledService was explicitly disabled but no service was generated with that name. Generated:\n ${
            baseModules.joinToString(
                "\n "
            )
            }"
        }
    }
    // validate inclusion list hits
    serviceMembership.inclusions.forEach { service ->
        check(baseModules.contains(service)) { "Service $service was in explicit inclusion list but not generated!" }
    }
    return baseServices.filter {
        serviceMembership.isMember(it.module)
    }
}

data class Membership(val inclusions: Set<String> = emptySet(), val exclusions: Set<String> = emptySet())

data class AwsService(
    val service: String,
    val module: String,
    val moduleDescription: String,
    val modelFile: File,
    val extraConfig: String? = null,
    val extraFiles: List<File>,
    val humanName: String
) {
    fun files(): List<File> = listOf(modelFile) + extraFiles
    fun Project.examples(): File = projectDir.resolve("examples").resolve(module)
    /**
     * Generate a link to the examples for a given service
     */
    fun examplesUri(project: Project) = if (project.examples().exists()) {
        "https://github.com/awslabs/aws-sdk-rust/tree/main/examples/$module"
    } else {
        null
    }
}

fun AwsService.crate(): String = "aws-sdk-$module"

fun Membership.isMember(member: String): Boolean = when {
    exclusions.contains(member) -> false
    inclusions.contains(member) -> true
    inclusions.isEmpty() -> true
    else -> false
}

fun parseMembership(rawList: String): Membership {
    val inclusions = mutableSetOf<String>()
    val exclusions = mutableSetOf<String>()

    rawList.split(",").map { it.trim() }.forEach { item ->
        when {
            item.startsWith('-') -> exclusions.add(item.substring(1))
            item.startsWith('+') -> inclusions.add(item.substring(1))
            else -> error("Must specify inclusion (+) or exclusion (-) prefix character to $item.")
        }
    }

    val conflictingMembers = inclusions.intersect(exclusions)
    require(conflictingMembers.isEmpty()) { "$conflictingMembers specified both for inclusion and exclusion in $rawList" }

    return Membership(inclusions, exclusions)
}
