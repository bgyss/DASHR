#include "ConstantBuffer.hlsl"
#include "VertexInput.hlsl"
#include "Utils.hlsl"

struct VS_OUTPUT
{
	float4 Pos : SV_POSITION;
	float3 TexCoord : TEXCOORD0;
	float3 StartInObject : TEXCOORD1;
	float3 DirectionInObject : TEXCOORD2;
};

VS_OUTPUT vs_main( VS_INPUT In )
{
	VS_OUTPUT Out;
	float4 posObject       = float4 ( In.Pos,       1.0f );
	float4 normObject      = float4 ( In.Normal,    0.0f );
	float4 tangentObject   = float4 ( In.Tangent,   0.0f );
	float4 bitangentObject = float4 ( In.Bitangent, 0.0f );

	Animate ( posObject,
			  normObject,
			  tangentObject,
			  bitangentObject,
			  In.BoneWeights );

	// And now extrude along the normal.
	// Mesh position is assumed to be where heightfield = 0.5, and typically In.TexCoord.z == 1.0, i.e. full extrusion.
	// We do need to extrude a little bit more to cope with non-linear interpolation artifacts,
	// i.e. the matrices/heightfield interpolate in a curvy way, while the mesh of course is made of flat triangles.
	posObject.xyz += normObject.xyz * In.TexCoord.z * ( HeightScale + HeightExtraMeshExtrude ) * 0.5f;

	// Note that tangentObject and bitangentObject are not used by this pass.
	// Nor are In.Tangent and In.Biangent. They could be removed from the vertex
	// data. However they are used for the distortion pass, and it is useful
	// to be able to use the same vertex buffer for both.
	// Additionally, if you switch back to conventional rendering,
	// you will also need them to do lighting.

	float4 cameraPos = mul(cameraFromObjectMatrix, posObject);
	float4 projectionPos = mul(projectionFromCameraMatrix, cameraPos);
	Out.Pos = projectionPos;
	Out.TexCoord  = In.TexCoord;

	// The direction vector is the line from the camera to the point.
	// In camera space, the camera is at the origin of course, so...
	float3 directionInCamera = cameraPos.xyz;
	float3 directionInObject = mul(objectFromCameraMatrix, float4(directionInCamera, 0.0f)).xyz;
	// Note - the direction should NOT be normalized here, it will be normalized
	// after interpolation in the pixel shader. Normalizing here will lead to
	// incorrect interpolation and "swimming" as the camera gets close.

	Out.StartInObject = posObject.xyz;
	Out.DirectionInObject = directionInObject;

	return Out;
}


Texture2D<float4> texAlbedo : register(t0);
Texture2D<float4> texHeight : register(t1);
Texture2D<float4> texNormal : register(t2);
Texture2D<float4> texSurfaceFromObject0 : register(t3);
Texture2D<float4> texSurfaceFromObject1 : register(t4);
Texture2D<float4> texSurfaceFromObject2 : register(t5);
Texture2D<float4> texSurfaceFromObject3 : register(t6);
Texture2D<float3> texTeleportMap : register(t7);
SamplerState smp : register (s0);
SamplerState smpPoint : register (s1);

float4x4 ReadSurfaceFromObject ( inout float2 animDistortion, float2 texCoords )
{
	float4 surfaceFromObject0 = texSurfaceFromObject0.SampleLevel ( smp, texCoords, 0.0 );
	float4 surfaceFromObject1 = texSurfaceFromObject1.SampleLevel ( smp, texCoords, 0.0 );
	float4 surfaceFromObject2 = texSurfaceFromObject2.SampleLevel ( smp, texCoords, 0.0 );
    float4 surfaceFromObject3 = texSurfaceFromObject3.SampleLevel ( smp, texCoords, 0.0 );
	
	animDistortion = float2 ( surfaceFromObject0.w, surfaceFromObject1.w );
	
	float4x4 surfaceFromObject;
    surfaceFromObject._m00 = surfaceFromObject0.x;
    surfaceFromObject._m10 = surfaceFromObject0.y;
	surfaceFromObject._m20 = surfaceFromObject0.z;
    surfaceFromObject._m30 = 0.0f;
    surfaceFromObject._m01 = surfaceFromObject1.x;
	surfaceFromObject._m11 = surfaceFromObject1.y;
	surfaceFromObject._m21 = surfaceFromObject1.z;
    surfaceFromObject._m31 = 0.0f;
	surfaceFromObject._m02 = surfaceFromObject2.x;
	surfaceFromObject._m12 = surfaceFromObject2.y;
	surfaceFromObject._m22 = surfaceFromObject2.z;
    surfaceFromObject._m32 = 0.0f;
    surfaceFromObject._m03 = surfaceFromObject3.x;
    surfaceFromObject._m13 = surfaceFromObject3.y;
    surfaceFromObject._m23 = surfaceFromObject3.z;
	surfaceFromObject._m33 = 1.0f;
	return surfaceFromObject;
}

float3x3 GetObjectFromSurfaceFromDistortionTexture ( inout float2 debugAnimDistortion, float2 texCoords )
{
	float4x4 surfaceFromObject = ReadSurfaceFromObject ( debugAnimDistortion, texCoords );
	
	// But we need the inverse for lighting :-(
	// Fortunately we don't need the offset vector, so pre-zero that.
	surfaceFromObject._m03 = 0.0f;
    surfaceFromObject._m13 = 0.0f;
    surfaceFromObject._m23 = 0.0f;
	surfaceFromObject._m33 = 1.0f;
	
	// This is the same code for DistortionMode=0 and DistortionMode=1

	float4x4 objectFromSurface = GetInverse4x4 ( surfaceFromObject );

    float3x3 objectFromSurface33;
    objectFromSurface33._m00 = objectFromSurface._m00;
    objectFromSurface33._m01 = objectFromSurface._m01;
    objectFromSurface33._m02 = objectFromSurface._m02;
    objectFromSurface33._m10 = objectFromSurface._m10;
    objectFromSurface33._m11 = objectFromSurface._m11;
    objectFromSurface33._m12 = objectFromSurface._m12;
    objectFromSurface33._m20 = objectFromSurface._m20;
    objectFromSurface33._m21 = objectFromSurface._m21;
    objectFromSurface33._m22 = objectFromSurface._m22;
    return objectFromSurface33;
}


float3 GetPosInSurfaceFromDistortionTexture ( inout float rayStepFactor,
											  float3 posInObject,
										      float2 texCoords )
{
	rayStepFactor = 1.0f;
	
	// DebugDampingFactor1 is usually 1.0f
	// DebugDampingFactor2 is usually 0.0f
	// DebugDampingFactor3 is usually 1.5f
	//		Factor3 is the only one chosen heuristically. Anything between 1.0 and 2.0 seems to work well.
		
	float2 animDistortion;
    float4x4 surfaceFromObject = ReadSurfaceFromObject ( animDistortion, texCoords );
	float2 scaleFactor;
	float2 rayStepFactor2D;
	bool identityScaling = true;
	if ( animDistortion.x < DebugDampingFactor2 )
    {
		// Highly compressed by animation and close to or causing self-intersection.
		// We want TraceRay to step very tiny distances in object space
		// until we can move the position in surface space past the area of
		// self-intersection and out the other side.
		scaleFactor.x = 1.0f;
		rayStepFactor2D.x = 0.01f;
    }
	else if ( animDistortion.x > DebugDampingFactor3 )
    {
		// Animation has caused a lot of stretching.
		// We need to damp the motion in surface space to prevent oscillation.
		scaleFactor.x = 1.0f / ( DebugDampingFactor1 * animDistortion.x );
		identityScaling = false;
		// ...and also step slower in object space.
		rayStepFactor2D.x = scaleFactor.x;
    }
	else
    {
		// Normal area with low animation distortion.
		scaleFactor.x = 1.0f;
		rayStepFactor2D.x = 1.0f;
    }
	
	// ...and the same with the V channel.
	if ( animDistortion.y < DebugDampingFactor2 )
    {
		scaleFactor.y = 1.0f;
		rayStepFactor2D.y = 0.01f;
    }
	else if ( animDistortion.y > DebugDampingFactor3 )
    {
		scaleFactor.y = 1.0f / ( DebugDampingFactor1 * animDistortion.y );
		identityScaling = false;
		rayStepFactor2D.y = scaleFactor.y;
    }
	else
    {
		scaleFactor.y = 1.0f;
		rayStepFactor2D.y = 1.0f;
    }
	
	rayStepFactor = min ( rayStepFactor2D.x, rayStepFactor2D.y );
	
	if ( identityScaling )
    {
		// scaleFactor.xy == 1.0f. Normal stepping with low animation distortion.
		// Common case, worth specialising.
		if ( DistortionMode == 0 )
		{
			// Treated just as a 4x3 matrix mapping one space to the other.
			float3 posInSurface = mul ( surfaceFromObject, float4 ( posInObject, 1.0f ) ).xyz;
			return posInSurface;
		}
		else //DistortionMode == 1
		{
			// The final vector of DistortionTexture stores where we EXPECTED to be in Object Space when it was sampled.
			// So find the error, which should hopefully be small, and mostly only in the vertical direction.
			float3 offsetInObject = posInObject - float3 ( surfaceFromObject._m03, surfaceFromObject._m13, surfaceFromObject._m23 );
		
			// Now transform the error to surface space, ignoring the offset we already used above.
			float3 offsetInSurface = mul(surfaceFromObject, float4(offsetInObject, 0.0f)).xyz;
		
			// And we know the UV offset - it's the middle of surface space where we sampled.
			float3 posInSurface = offsetInSurface + float3 ( texCoords.x, texCoords.y, 0.5f );
			return posInSurface;
		}
    }
	else
    {
        if ( DistortionMode == 0 )
        {
            float3 posInSurface = mul ( surfaceFromObject, float4 ( posInObject, 1.0f ) ).xyz;
            float2 offset = posInSurface.xy - texCoords;
            texCoords += offset * scaleFactor;
            posInSurface.xy = texCoords;
            return posInSurface;
        }
        else
        {
            float3 offsetInObject = posInObject - float3 ( surfaceFromObject._m03, surfaceFromObject._m13, surfaceFromObject._m23 );
            float3 offsetInSurface = mul ( surfaceFromObject, float4 ( offsetInObject, 0.0f ) ).xyz;
            float3 posInSurface = offsetInSurface * float3 ( scaleFactor.x, scaleFactor.y, 1.0f ) + float3 ( texCoords.x, texCoords.y, 0.5f );
            return posInSurface;
        }
    }
}

// Return is false if it escaped, true if it hit something.
bool TraceRay ( inout int debugNumSteps,
				inout int debugNumTeleports,
				inout float3 posInSurface,
				inout float objectDistance,
				float3 startInObject,
				float3 dirInObject,
				float heightScale,
				float heightOffset,
				float stepSize,
				float stepScale
			  )
{
	debugNumSteps = 0;
	objectDistance = 0.0f;
	float3 posInObject = startInObject;

	float height = 0.0f;
	float deltaHeight = -1.0f;

	// heightScale actually scales towards 0.5.
	float envelopeMin = ( 0.0f - 0.5f ) * heightScale + 0.5f + heightOffset - 0.5f;
	float envelopeMax = ( 1.0f - 0.5f ) * heightScale + 0.5f + heightOffset + 0.5f;

	float prevObjectDistance = objectDistance;
	float thisStepSize = 0.0f;

	while(debugNumSteps < 10000) // hardwired timeout to stop Windows killing the program if you have a bug.
	{
		prevObjectDistance = objectDistance;		
		float3 prevPosInSurface = posInSurface;
		float3 prevPosInObject = posInObject;
		float prevHeight = height;
		float prevDeltaHeight = deltaHeight;

		if ( ( DebugMode == 2 ) || ( MaxSteps == debugNumSteps ) ) // MaxSteps == -1 means "disabled"
		{
			// Hit immediately.
			return true;
		}
		
		// Do we need to teleport anywhere?
		// First sample WITH filtering, so the SDF can be interpolated.
		// It may make sense to separate the teleport SDF and the teleport destination UVs
		// into separate textures, since the SDF is always read, but the destination is only read
		// when a telelport is actually required.
		float teleportSdf = texTeleportMap.SampleLevel ( smp, posInSurface.xy, 0.0 ).z;
		bool teleported = false;
		if ( teleportSdf > 0.0f )
        {
			// The teleport destination is POINT sampled (because it has discontinuitiies, so filtering won't work)
			// Subtlety here - because the teleport is POINT sampled, the new posInSurface
			// is not very accurate - it could be up to a texel off. For this reason it
			// is important to call GetPosInSurfaceFromDistortionTexture() AFTER the teleport,
			// so it can fix it up posInSurface and get more precision.
			float2 teleportDest = texTeleportMap.SampleLevel ( smpPoint, posInSurface.xy, 0.0 ).xy;
			posInSurface.xy = teleportDest;
			debugNumTeleports++;
			teleported = true;

			for ( int i = 0; i < DebugIterationsAfterTeleport; i++ ) // typically DebugIterationsAfterTeleport = 0
            {
				float rayStepFactor;
				posInSurface = GetPosInSurfaceFromDistortionTexture ( rayStepFactor, posInObject, posInSurface.xy );
            }
        }

		// Do the step. Note this may be shortened later.
		objectDistance = prevObjectDistance + thisStepSize;
		// Calculate this fresh every time to avoid accumulating error.
		posInObject = startInObject + dirInObject * objectDistance;
		
		// We step the ray in object space, but have to convert to
		// surface space to sample the distortion and heightfield.
		// To do this we need to get the local surfaceFromObject matrix.
		// However, this is stored in SURFACE space, so we have a chicken-and-egg problem.
		// So we use the last surface position to sample the distortion,
		// and hope this is close enough to get a reasonable answer.
		// As we get closer to actual intersection, we take smaller steps anyway,
		// and hopefully it should all just fix itself that way.
		float rayStepFactor;
		posInSurface = GetPosInSurfaceFromDistortionTexture ( rayStepFactor, posInObject, posInSurface.xy );
		if ( rayStepFactor < 1.0f )
        {
			// rayStepFactor < 1.0f means that the local animation distortion was high enough
			// that we needed to take a shorter step to avoid oscillation.
			rayStepFactor = max ( 0.0001f, rayStepFactor );
			objectDistance = prevObjectDistance + thisStepSize * rayStepFactor;
			posInObject = startInObject + dirInObject * objectDistance;
        }
		
		if ( ( posInSurface.x < 0.0f ) ||
			 ( posInSurface.x > 1.0f ) ||
			 ( posInSurface.y < 0.0f ) ||
			 ( posInSurface.y > 1.0f ) ||
			 ( posInSurface.z < envelopeMin ) ||
			 ( posInSurface.z > envelopeMax ) )
		{
			// Escaped the envelope.
			return false;
		}

		float texelHeight = texHeight.SampleLevel ( smp, posInSurface.xy, 0.0 ).r;
		// heightScale actually scales towards 0.5.
		height = ( texelHeight - 0.5f ) * heightScale + 0.5f + heightOffset;
		deltaHeight = height - posInSurface.z;

		if ( deltaHeight > 0.0f )
		{
			// Hit the surface.
			if ( !teleported ) // interpolation over a teleport doesn't work of course!
			{
				// Interpolate backwards along the ray in surface space to get a more precise intersection.
				// This allows larger steps without looking "lumpy"
				// To do even better you'd do a binary chop of the space to zero in on the exact intersection point.
				float deltaDelta = deltaHeight - prevDeltaHeight;
				if (abs(deltaDelta) > 0.00001f)
				{
					float lambda = deltaHeight / deltaDelta;
					// Wrapping can cause instant deltas in texel height, so clamp how far we can interpolate back.
					lambda = clamp ( lambda, 0.0, 1.0f );
					float omLambda = 1.0f - lambda;
					posInSurface   = (prevPosInSurface   * lambda) + (posInSurface   * omLambda);
					posInObject    = (prevPosInObject    * lambda) + (posInObject    * omLambda);
					objectDistance = (prevObjectDistance * lambda) + (objectDistance * omLambda);
				}
			}
			return true;
		}

		// Figure out what the step size should be for next loop.
		float stepSkip = max ( 1.0, -deltaHeight * stepScale );
		thisStepSize = stepSize * stepSkip;
		
		debugNumSteps++;
	}

	// If we got here, we hit the step limit.
	return false;
}

// .x component is the direct light, .y is the indirect light.
float2 DoLighting ( float3 normalInObject )
{
	// This does "wrap round gouraud" which looks prettier and shows more shape than standard clamp(N.L)
	float nDotL = dot ( normalInObject, SunDirInObject );

	// Standard NdotL direct lighting will be shadowed
	float direct = clamp ( nDotL, 0.0f, 1.0f );

	// Indirect lighting will not be shadowed.
	float totalIndirect = IndirectLighting * clamp ( nDotL * 0.5f + 0.5f, 0.0f, 1.0f );

	return float2 ( clamp ( direct - totalIndirect, 0.0f, 1.0f ), totalIndirect );
}

float4 ps_main( VS_OUTPUT In ) : SV_TARGET
{
	float4 result = float4 ( 0.0f, 0.0f, 0.0f, 0.0f );

	float4x4 surfaceFromObject;

	float3 posInSurface = In.TexCoord.xyz;

	if (DebugMode==1)
	{
		return texHeight.SampleLevel ( smp, posInSurface.xy, 0.0f );
	}

	float3 startInObject = In.StartInObject;
	// See note in VS why we normalized after interpolation rather than before.
	float3 dirInObject = normalize ( In.DirectionInObject );

	int debugNumSteps = 0;
	int debugNumTeleports = 0;
	int debugNumShadowSteps = 0;
	int debugNumShadowTeleports = 0;

	float objectDistance = 0.0f;
 
	bool rayHit = TraceRay ( debugNumSteps, debugNumTeleports,
							 posInSurface,
							 objectDistance,
							 startInObject, dirInObject,
							 HeightScale, HeightOffset,
							 StepSize, StepScale );

	float3 posInObject = startInObject + dirInObject * objectDistance;
	float3 albedoPosInSurface = posInSurface;

	if (!rayHit)
	{
		result = float4 ( 0.0f, 0.0f, 0.0f, 0.0f );
	}
	else
	{
		// LightingMode:
		// 0 = none (and show heightfield)
		// 1 = surface normal without heightfield
		// 2 = heightfield deltas.
		// 3 = precomputed normal map.
		// 4 = shadow raytrace

		float3 albedo = texAlbedo.Sample ( smp, posInSurface.xy ).rgb;

		// Extract local tangent-space info.
		float2 debugAnimDistortion;
		float3x3 objectFromSurface = GetObjectFromSurfaceFromDistortionTexture ( debugAnimDistortion, posInSurface.xy );
		float3 tangentBasisObject   = float3 ( objectFromSurface._m00, objectFromSurface._m10, objectFromSurface._m20 );
		float3 bitangentBasisObject = float3 ( objectFromSurface._m01, objectFromSurface._m11, objectFromSurface._m21 );
		float3 normBasisObject      = float3 ( objectFromSurface._m02, objectFromSurface._m12, objectFromSurface._m22 );
		
		if ( LightingMode == 0 )
		{
			// No lighting, also shows the heightfield, not albedo.
			result.rgb = texHeight.SampleLevel ( smp, posInSurface.xy, 0.0f ).rgb;
		}
		else if ( DebugMode == 2 )
		{
			// No lighting, shows raw albedo.
			result.rgb = texAlbedo.Sample ( smp, posInSurface.xy ).rgb;
		}
		else if ( DebugMode == 5 )
        {
			// "no distortion" = a ratio of 1.0, but it can even go negative!
			debugAnimDistortion = 0.5f + ( debugAnimDistortion - 1.0f ) * 0.25f;
			result = float4 ( debugAnimDistortion.x, debugAnimDistortion.y, 0.0f, 1.0f );
        }
		else
		{
			float3 normalObject;
			if ( LightingMode == 1 )
			{
				normalObject = normalize ( normBasisObject );
			}
			else if ( LightingMode == 2 )
			{
				// Take deltas on the UV map to find the heightfield normal.
				// Tune deltaUVStep this to look tolerable - it should be comparable to the size of a texel on the heightfield.
				float baseHeight = texHeight.SampleLevel ( smp, posInSurface.xy, 0.0f ).r;
				float heightPU = texHeight.SampleLevel ( smp, posInSurface.xy + float2 (  DeltaUVStep, 0.0f ), 0.0f ).r;
				float heightNU = texHeight.SampleLevel ( smp, posInSurface.xy + float2 ( -DeltaUVStep, 0.0f ), 0.0f ).r;
				float heightPV = texHeight.SampleLevel ( smp, posInSurface.xy + float2 ( 0.0f,  DeltaUVStep ), 0.0f ).r;
				float heightNV = texHeight.SampleLevel ( smp, posInSurface.xy + float2 ( 0.0f, -DeltaUVStep ), 0.0f ).r;
				float deltaU = heightNU - heightPU;
				float deltaV = heightNV - heightPV;

				// This is somewhat hacky to scale the deltas up to be significant relative to the normal.
				float deltaUVScale = 0.25f * HeightNormalsScale / DeltaUVStep;

				// Then these are the actual vectors that point along the U & V *heightfield surface* (not the surface space)
				float3 heightfieldTangentObject   = tangentBasisObject   - deltaU * deltaUVScale * normBasisObject;
				float3 heightfieldBitangentObject = bitangentBasisObject - deltaV * deltaUVScale * normBasisObject;
				float3 heightfieldNormalObject = cross ( heightfieldTangentObject, heightfieldBitangentObject );
				normalObject = normalize ( heightfieldNormalObject );
			}
			else
			{
				// Standard normal map.
				float3 normalSurface = texNormal.Sample ( smp, posInSurface.xy, 0.0f ).rgb;
				normalSurface.x = normalSurface.x * HeightNormalsScale;
				normalSurface.y = normalSurface.y * HeightNormalsScale;
				normalObject = mul ( objectFromSurface, normalSurface );
				normalObject = normalize ( normalObject );
			}

			// Do we want shadows?
			// .x is direct, .y is indirect.
			float2 brightness = DoLighting ( normalObject );
			float directLight = brightness.x;
			float indirectLight = brightness.y;
			if ( LightingMode >= 4 )
			{
				if ( directLight > 0.0f )
				{
					float shadowDistance = 0.0f;
					// A cheap trick to avoid shadow acne is to move the ray origin away from the surface
					// *along its normal* (not towards the sun - that doesn't fix glancing angles)
					float3 startInObject = posInObject + ShadowAcneScaler * normalize ( normalObject );
					dirInObject = SunDirInObject;
					float debugRayStepFactor;
					posInSurface = GetPosInSurfaceFromDistortionTexture ( debugRayStepFactor, startInObject, posInSurface.xy );
					bool hitShadow = TraceRay ( debugNumShadowSteps, debugNumShadowTeleports,
												posInSurface,
												shadowDistance,
												startInObject, dirInObject,
												HeightScale, HeightOffset,
												StepSize, StepScale );
					if ( hitShadow )
					{
						directLight = 0.0f;
					}
				}
			}

			result.rgb = albedo * ( directLight + indirectLight );
		}

		result.w = 1.0f;
	}

	if (DebugMode==3)
	{
		result.r = (float)debugNumShadowSteps * 0.01f;
		result.g = (float)debugNumSteps * 0.01f;
		result.b = (float)debugNumTeleports * 0.1f;
		result.a = 1.0f;
	}
	else if (DebugMode == 4)
	{
		if (rayHit)
		{
			float teleportSdf = texTeleportMap.SampleLevel ( smp, albedoPosInSurface.xy, 0.0 ).z;
			
			result.rg = albedoPosInSurface.xy;
			result.b = ( teleportSdf > -0.01f ) ? 1.0f : 0.0f;
			result.a = 1.0f;
			if ( frac(albedoPosInSurface.x * 16.0f) < 0.01f ) { result.r += 0.2f; }
			if ( frac(albedoPosInSurface.y * 16.0f) < 0.01f ) { result.g += 0.2f; }
			if ( frac(albedoPosInSurface.x * 256.0f) < 0.08f ) { result.r += 0.2f; }
			if ( frac(albedoPosInSurface.y * 256.0f) < 0.08f ) { result.g += 0.2f; }
		}
	}

	if ( result.a <= 0.0f )
	{
		// "alpha test" also avoids writing depth.
		discard;
	}

	return result;
}

