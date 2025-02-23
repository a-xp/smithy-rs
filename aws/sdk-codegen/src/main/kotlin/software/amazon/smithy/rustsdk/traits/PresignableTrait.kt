/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0.
 */

package software.amazon.smithy.rustsdk.traits

import software.amazon.smithy.model.node.Node
import software.amazon.smithy.model.shapes.ShapeId
import software.amazon.smithy.model.traits.AnnotationTrait

/** Synthetic trait that indicates an operation is presignable. */
// TODO: This can be replaced once https://github.com/awslabs/smithy/pull/897 is merged.
class PresignableTrait(val syntheticOperationId: ShapeId) : AnnotationTrait(ID, Node.objectNode()) {
    companion object {
        val ID = ShapeId.from("smithy.api.aws.internal#presignable")
    }
}
